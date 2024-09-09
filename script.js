// 선택된 아이템 ID
let ITEM_ID = null;

// API 엔드포인트
const API_URL = "http://localhost:3000";

// DOM 요소들
const itemListEl = document.getElementById("itemList");
const itemDetailsEl = document.getElementById("itemDetails");
const itemIdEl = document.getElementById("itemId");
const itemNameEl = document.getElementById("itemName");
const itemDescriptionEl = document.getElementById("itemDescription");
const itemCurrentPriceEl = document.getElementById("itemCurrentPrice");
const itemBuyNowPriceEl = document.getElementById("itemBuyNowPrice");
const itemStatusEl = document.getElementById("itemStatus");
const itemEndTimeEl = document.getElementById("itemEndTime");
const itemTimeLeftEl = document.getElementById("itemTimeLeft");
const bidAmountEl = document.getElementById("bidAmount");
const bidderIdEl = document.getElementById("bidderId");
const placeBidBtn = document.getElementById("placeBidBtn");
const buyNowBtn = document.getElementById("buyNowBtn");
const itemBidHistoryEl = document.getElementById("itemBidHistory");
const itemStartTimeEl = document.getElementById("itemStartTime");

let countdownInterval;
let isUpdating = false;

// 전역 변수로 아이템 목록 저장
let itemsList = [];

// 아이템 목록 가져오기 및 노출
async function fetchAndDisplayItems() {
  try {
    const response = await fetch(`${API_URL}/items`);
    itemsList = await response.json();
    displayItems();
  } catch (error) {
    console.error("Failed to fetch items:", error);
  }
}

// 아이템 목록 표시
function displayItems() {
  itemListEl.innerHTML = "";
  itemsList.forEach((item) => {
    // 아이템 상태 업데이트
    updateItemStatus(item);

    const li = document.createElement("li");
    li.textContent = `${item.title} (ID: ${item.id}) - ${item.description} - 상태: ${item.status}`;
    li.addEventListener("click", () => showItemDetails(item.id));
    itemListEl.appendChild(li);
  });
}

// 아이템 상태 업데이트
function updateItemStatus(item) {
  if (item.status === "COMPLETED") {
    return;
  }

  const now = new Date();
  const startTime = new Date(item.start_time);
  const endTime = new Date(item.end_time);

  if (now > endTime) {
    item.status = "COMPLETED";
  } else if (now < startTime) {
    item.status = "SCHEDULED";
  } else {
    item.status = "ACTIVE";
  }
}

// 주기적으로 상품 목록 상태 갱신
function startItemListRefresh() {
  setInterval(() => {
    itemsList.forEach(updateItemStatus);
    displayItems();
  }, 1000); // 1초마다 갱신
}

// 초기 데이터 로드 및 주기적 갱신 시작
fetchAndDisplayItems().then(startItemListRefresh);

// 아이템 상세 보기
async function showItemDetails(itemId) {
  if (isUpdating) return;
  isUpdating = true;

  try {
    const [itemResponse, bidsResponse] = await Promise.all([
      fetch(`${API_URL}/items/${itemId}`),
      fetch(`${API_URL}/items/${itemId}/bids`),
    ]);

    const item = await itemResponse.json();
    const bids = await bidsResponse.json();

    // 아이템 상태 업데이트
    updateItemStatus(item);

    // 전역 아이템 목록 업데이트
    const index = itemsList.findIndex((i) => i.id === item.id);
    if (index !== -1) {
      itemsList[index] = item;
    }

    // ITEM_ID를 선택한 아이템의 ID로 업데이트
    ITEM_ID = item.id;

    // UI 업데이트 로직
    updateItemDetailsUI(item, bids);

    // 경매 상태에 따른 UI 업데이트
    updateAuctionStatusUI(item);

    if (itemDetailsEl) itemDetailsEl.style.display = "block";

    // 상품 목록 갱신
    displayItems();
  } catch (error) {
    console.error("Failed to fetch item details:", error);
  } finally {
    isUpdating = false;
  }
}

// UI 업데이트 함수
function updateItemDetailsUI(item, bids) {
  if (itemIdEl) itemIdEl.textContent = item.id;
  if (itemNameEl) itemNameEl.textContent = item.title;
  if (itemDescriptionEl) itemDescriptionEl.textContent = item.description;
  if (itemCurrentPriceEl) itemCurrentPriceEl.textContent = item.current_price;
  if (itemBuyNowPriceEl) itemBuyNowPriceEl.textContent = item.buy_now_price;
  if (itemStatusEl) itemStatusEl.textContent = item.status;

  // 입찰 기록 표시 로직
  updateBidHistory(bids);

  // 시작 시간 표시
  if (itemStartTimeEl && item.start_time) {
    const startTime = new Date(item.start_time);
    itemStartTimeEl.textContent = startTime.toLocaleString();
  }

  // 종료 시간 및 카운트다운 설정
  if (itemEndTimeEl && item.end_time) {
    const endTime = new Date(item.end_time);
    itemEndTimeEl.textContent = endTime.toLocaleString();

    clearInterval(countdownInterval);
    updateCountdown(endTime);
    countdownInterval = setInterval(() => updateCountdown(endTime), 1000);
  }
}

// 경매 상태에 따른 UI 업데이트
function updateAuctionStatusUI(item) {
  const now = new Date();
  const startTime = new Date(item.start_time);
  const endTime = new Date(item.end_time);
  let statusMessage = "";

  if (item.status === "COMPLETED" || (endTime && now > endTime)) {
    statusMessage = "경매가 종료되었습니다.";
    item.status = "COMPLETED";
  } else if (startTime && now < startTime) {
    statusMessage = "경매가 아직 시작되지 않았습니다.";
  } else if (item.status !== "ACTIVE") {
    statusMessage = "경매가 활성 상태가 아닙니다.";
  }

  // 상태 메시지 업데이트
  const statusMessageEl = document.getElementById("auctionStatusMessage");
  if (statusMessageEl) {
    statusMessageEl.textContent = statusMessage;
  }

  // 상태가 COMPLETED일 때 입력 필드와 버튼을 비활성화합니다.
  const isCompleted = item.status === "COMPLETED";
  if (placeBidBtn) placeBidBtn.disabled = isCompleted;
  if (buyNowBtn) buyNowBtn.disabled = isCompleted;
  if (bidAmountEl) bidAmountEl.disabled = isCompleted;
  if (bidderIdEl) bidderIdEl.disabled = isCompleted;

  // 아이템 상태 업데이트
  if (itemStatusEl) itemStatusEl.textContent = item.status;
}

// 카운트다운 업데이트
function updateCountdown(endTime) {
  if (!itemTimeLeftEl) return;

  const now = new Date();
  const timeLeft = endTime - now;

  if (timeLeft <= 0) {
    itemTimeLeftEl.textContent = "경매 종료";
    clearInterval(countdownInterval);
    // 경매가 종료되면 UI만 업데이트합니다.
    if (!isUpdating) {
      isUpdating = true;
      const item = itemsList.find((i) => i.id === ITEM_ID);
      if (item) {
        item.status = "COMPLETED";
        updateAuctionStatusUI(item);
        displayItems();
      }
      isUpdating = false;
    }
  } else {
    const days = Math.floor(timeLeft / (1000 * 60 * 60 * 24));
    const hours = Math.floor(
      (timeLeft % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60)
    );
    const minutes = Math.floor((timeLeft % (1000 * 60 * 60)) / (1000 * 60));
    const seconds = Math.floor((timeLeft % (1000 * 60)) / 1000);

    itemTimeLeftEl.textContent = `${days}일 ${hours}시간 ${minutes}분 ${seconds}초`;
  }
}

// 입찰하기
async function placeBid() {
  if (ITEM_ID === null) {
    alert("먼저 아이템을 선택해주세요.");
    return;
  }

  if (itemStatusEl.textContent !== "ACTIVE") {
    alert("현재 경매가 활성 상태가 아닙니다.");
    return;
  }

  const startTime = new Date(itemStartTimeEl.textContent);
  if (isNaN(startTime.getTime())) {
    alert("유효한 경매 시작 시간을 불러올 수 없습니다.");
    return;
  }

  const now = new Date();
  if (now < startTime) {
    alert("경매가 아직 시작되지 않았습니다.");
    return;
  }

  const bid_amount = parseInt(bidAmountEl.value);
  const bidder_id = bidderIdEl.value;
  const buy_now_price = parseInt(itemBuyNowPriceEl.textContent);

  if (!bidder_id) {
    alert("입찰자 ID를 입력해주세요.");
    return;
  }

  if (bid_amount >= buy_now_price) {
    const confirmBuyNow = confirm(
      "입찰가가 즉시 구매가 이상입니다. 즉시 구매하시겠습니까?"
    );
    if (confirmBuyNow) {
      buyNow();
      return;
    } else {
      return;
    }
  }

  try {
    const response = await fetch(`${API_URL}/bid`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        item_id: ITEM_ID,
        bidder_id: parseInt(bidder_id),
        bid_amount: bid_amount,
      }),
    });

    const data = await response.json();

    if (response.ok) {
      alert(data.message);
      showItemDetails(ITEM_ID);
      console.log("입찰 성공");
    } else {
      handleBidError(data);
    }
  } catch (error) {
    console.error("입찰 실패:", error);
    alert("입찰 중 오류가 발생했습니다. 다시 시도해주세요.");
  }
}

// 입찰 오류 처리
function handleBidError(data) {
  switch (data.code) {
    case "LOW_BID":
      alert(
        "입찰 금액이 현재 가격보다 낮습니다. 더 높은 금액으로 입찰해주세요."
      );
      break;
    case "NOT_STARTED":
      alert("경매가 아직 시작되지 않았습니다. 경매 시작 시간을 확인해주세요.");
      break;
    case "ALREADY_ENDED":
      alert("경매가 이미 종료되었습니다. 다른 경매에 참여해주세요.");
      break;
    default:
      alert(`입찰 실패: ${data.error}`);
  }
}

// 즉시 구매
async function buyNow() {
  if (ITEM_ID === null) {
    alert("먼저 아이템을 선택해주세요.");
    return;
  }

  // 경매 상태 확인
  if (itemStatusEl.textContent !== "ACTIVE") {
    alert("현재 경매가 활성 상태가 아닙니다.");
    return;
  }

  // 경매 시작 시간 확인
  const startTime = new Date(itemStartTimeEl.textContent);
  if (isNaN(startTime.getTime())) {
    alert("유효한 경매 시작 시간을 불러올 수 없습니다.");
    return;
  }

  const now = new Date();
  if (now < startTime) {
    alert("경매가 아직 시작되지 않았습니다.");
    return;
  }

  const buyerId = bidderIdEl.value;

  if (!buyerId) {
    alert("구매자 ID를 입력해주세요.");
    return;
  }

  try {
    const response = await fetch(`${API_URL}/buy-now`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        item_id: ITEM_ID,
        buyer_id: parseInt(buyerId),
      }),
    });

    if (response.ok) {
      alert("즉시 구매 성공!");
      showItemDetails(ITEM_ID);
    } else {
      const errorData = await response.json();
      alert(
        `즉시 구매 실패: ${
          errorData.error || "알 수 없는 오류가 발생했습니다."
        }`
      );
    }
  } catch (error) {
    console.error("즉시 구매 실패:", error);
    alert("즉시 구매 중 오류가 발생했습니다. 다시 시도해주세요.");
  }
}

// 이벤트 리스너 등록
placeBidBtn.addEventListener("click", placeBid);
buyNowBtn.addEventListener("click", buyNow);

// 입찰 기록 업데이트
function updateBidHistory(bids) {
  if (!itemBidHistoryEl) return;

  // 입찰 기록 요소 초기화
  itemBidHistoryEl.innerHTML = "";

  // 입찰 기록이 없는 경우
  if (bids.length === 0) {
    itemBidHistoryEl.innerHTML = "<p>아직 입찰 기록이 없습니다.</p>";
    return;
  }

  // 입찰 기록 테이블 생성
  const table = document.createElement("table");
  table.innerHTML = `
    <thead>
      <tr>
        <th>입찰자 ID</th>
        <th>입찰 금액</th>
        <th>입찰 시간</th>
      </tr>
    </thead>
    <tbody>
    </tbody>
  `;

  const tbody = table.querySelector("tbody");

  // 입찰 기록을 최신순으로 정렬
  bids.sort((a, b) => new Date(b.bid_time) - new Date(a.bid_time));

  // 각 입찰 기록을 테이블에 추가
  bids.forEach((bid) => {
    const row = document.createElement("tr");
    row.innerHTML = `
      <td>${bid.bidder_id}</td>
      <td>${bid.bid_amount}</td>
      <td>${new Date(bid.bid_time).toLocaleString()}</td>
    `;
    tbody.appendChild(row);
  });

  // 테이블을 입찰 기록 요소에 추가
  itemBidHistoryEl.appendChild(table);
}
